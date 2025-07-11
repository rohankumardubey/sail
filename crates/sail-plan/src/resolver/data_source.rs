use std::collections::HashSet;

use datafusion::arrow::datatypes as adt;
use datafusion::datasource::listing::{ListingOptions, ListingTableUrl};
use datafusion_common::plan_err;
use futures::{StreamExt, TryStreamExt};
use sail_common::spec;

use crate::error::PlanResult;
use crate::resolver::state::PlanResolverState;
use crate::resolver::url::GlobUrl;
use crate::resolver::PlanResolver;

impl PlanResolver<'_> {
    pub(super) async fn resolve_listing_urls(
        &self,
        paths: Vec<String>,
    ) -> PlanResult<Vec<ListingTableUrl>> {
        let mut urls = vec![];
        for path in paths {
            for url in GlobUrl::parse(&path)? {
                let url = self.rewrite_directory_url(url).await?;
                urls.push(url.try_into()?);
            }
        }
        Ok(urls)
    }

    pub(super) async fn resolve_listing_schema(
        &self,
        urls: &[ListingTableUrl],
        options: &ListingOptions,
        schema: Option<spec::Schema>,
        state: &mut PlanResolverState,
    ) -> PlanResult<adt::Schema> {
        let schema = match schema {
            // ignore empty schema
            Some(spec::Schema { fields }) if fields.is_empty() => None,
            x => x,
        };
        // The logic is similar to `ListingOptions::infer_schema()`
        // but here we also check for the existence of files.
        let session_state = self.ctx.state();
        let mut file_groups = vec![];
        for url in urls {
            let store = self.ctx.runtime_env().object_store(url)?;
            let files: Vec<_> = url
                .list_all_files(&session_state, &store, &options.file_extension)
                .await?
                // Here we sample up to 10 files to infer the schema.
                // The value is hard-coded here since DataFusion uses the same hard-coded value
                // for operations such as `infer_partitions_from_path`.
                // We can make it configurable if DataFusion makes those operations configurable
                // as well in the future.
                .take(10)
                .try_collect()
                .await?;
            file_groups.push((store, files));
        }
        let empty = file_groups.iter().all(|(_, files)| files.is_empty());
        if empty {
            let urls = urls
                .iter()
                .map(|url| url.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return plan_err!("No files found in the specified paths: {urls}")?;
        }
        let schema = match schema {
            Some(schema) => self.resolve_schema(schema, state)?,
            None => {
                let mut schemas = vec![];
                for (store, files) in file_groups.iter() {
                    let mut schema = options
                        .format
                        .infer_schema(&session_state, store, files)
                        .await?;
                    let ext = options.format.get_ext().to_lowercase();
                    let ext = ext.trim();
                    if matches!(ext, ".csv") || matches!(ext, "csv") {
                        schema = rename_default_csv_columns(schema);
                    }
                    schemas.push(schema.as_ref().clone());
                }
                adt::Schema::try_merge(schemas)?
            }
        };

        // FIXME: DataFusion 43.0.0 suddenly doesn't support Utf8View
        let new_fields: Vec<adt::Field> = schema
            .fields()
            .iter()
            .map(|field| {
                if matches!(field.data_type(), &adt::DataType::Utf8View) {
                    let mut new_field = field.as_ref().clone();
                    new_field = new_field.with_data_type(adt::DataType::Utf8);
                    new_field
                } else {
                    field.as_ref().clone()
                }
            })
            .collect();
        Ok(adt::Schema::new_with_metadata(
            new_fields,
            schema.metadata().clone(),
        ))
    }
}

fn rename_default_csv_columns(schema: adt::SchemaRef) -> adt::SchemaRef {
    let mut failed_parsing = false;
    let mut seen_names = HashSet::new();
    let mut new_fields = schema
        .fields()
        .iter()
        .map(|field| {
            // Order may not be guaranteed, so we try to parse the index from the column name
            let new_name = if field.name().starts_with("column_") {
                if let Some(index_str) = field.name().strip_prefix("column_") {
                    if let Ok(index) = index_str.trim().parse::<usize>() {
                        format!("_c{}", index.saturating_sub(1))
                    } else {
                        failed_parsing = true;
                        field.name().to_string()
                    }
                } else {
                    field.name().to_string()
                }
            } else {
                field.name().to_string()
            };
            if !seen_names.insert(new_name.clone()) {
                failed_parsing = true;
            }
            adt::Field::new(new_name, field.data_type().clone(), field.is_nullable())
        })
        .collect::<Vec<_>>();

    if failed_parsing {
        new_fields = schema
            .fields()
            .iter()
            .enumerate()
            .map(|(i, field)| {
                adt::Field::new(
                    format!("_c{i}"),
                    field.data_type().clone(),
                    field.is_nullable(),
                )
            })
            .collect::<Vec<_>>();
    }

    adt::SchemaRef::new(adt::Schema::new_with_metadata(
        new_fields,
        schema.metadata().clone(),
    ))
}
