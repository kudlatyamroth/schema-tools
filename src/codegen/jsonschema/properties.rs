use super::{
    types::{AnyType, FlatModel, Model, ModelType, NullableOptionalWrapperType, ObjectType},
    JsonSchemaExtractOptions, ModelContainer,
};
use crate::{error::Error, resolver::SchemaResolver, scope::SchemaScope};
use serde_json::{Map, Value};

pub fn from_object_with_properties(
    schema: &Map<String, Value>,
    container: &mut ModelContainer,
    scope: &mut SchemaScope,
    resolver: &SchemaResolver,
    options: &JsonSchemaExtractOptions,
) -> Result<Model, Error> {
    let name = super::title::extract_title(&schema, scope, options)?;
    let required = super::required::extract_required(&schema, scope);

    match schema.get("properties") {
        Some(Value::Object(props)) => {
            scope.form("properties");

            let properties = props
                .iter()
                .map(|(name, property)| {
                    scope.property(name);

                    let mut model =
                        super::extract_type(property, container, scope, resolver, options)
                            .and_then(|s| s.flatten(container, scope))
                            .map_err(|e| {
                                scope.pop();
                                e
                            })?;

                    model.name = Some(name.clone());
                    model.attributes.required = required.contains(name);

                    let model = if model.attributes.nullable
                        && !model.attributes.required
                        && options.optional_and_nullable_as_models
                    {
                        convert_to_nullable_optional_wrapper(model, container, scope).map_err(
                            |e| {
                                scope.pop();
                                e
                            },
                        )?
                    } else {
                        model
                    };

                    scope.pop();

                    Ok(model)
                })
                .collect::<Result<Vec<FlatModel>, Error>>()?;

            scope.pop();

            Ok(Model::new(ModelType::ObjectType(ObjectType {
                name,
                properties,
            })))
        }
        _ => Err(Error::SchemaInvalidProperty("properties".to_string())),
    }
}

fn convert_to_nullable_optional_wrapper(
    mut model: FlatModel,
    container: &mut ModelContainer,
    scope: &mut SchemaScope,
) -> Result<FlatModel, Error> {
    model.attributes.required = true;
    model.attributes.nullable = false;

    let wrapper = Model::new(ModelType::NullableOptionalWrapperType(
        NullableOptionalWrapperType {
            model,
            name: scope.namer().decorate(vec!["optional".to_string()]),
        },
    ));

    wrapper.flatten(container, scope)
}

pub fn from_object(
    schema: &Map<String, Value>,
    container: &mut ModelContainer,
    scope: &mut SchemaScope,
    resolver: &SchemaResolver,
    options: &JsonSchemaExtractOptions,
) -> Result<Model, Error> {
    from_object_with_properties(schema, container, scope, resolver, options)
        .or_else(|_| {
            super::additionalproperties::from_object_with_additional_properties(
                schema, container, scope, resolver, options,
            )
        })
        .or_else(|_| Ok(AnyType::model(schema, scope)))
}

#[cfg(test)]
mod tests {
    use crate::codegen::jsonschema::types::Attributes;

    use super::*;
    use serde_json::json;

    #[test]
    fn test_should_convert_to_object() {
        let schema = json!({
            "required": ["a"],
            "properties": {
                "a": { "type": "string"},
                "b": { "type": "number"}
            }
        });

        let mut container = ModelContainer::default();
        let mut scope = SchemaScope::default();
        let resolver = SchemaResolver::empty();
        let options = JsonSchemaExtractOptions::default();

        scope.entity("TestName");
        let result = from_object_with_properties(
            schema.as_object().unwrap(),
            &mut container,
            &mut scope,
            &resolver,
            &options,
        );

        assert_eq!(
            result.unwrap(),
            Model::new(ModelType::ObjectType(ObjectType {
                name: "TestName".to_string(),
                properties: vec![
                    FlatModel {
                        name: Some("a".to_string()),
                        type_: "string".to_string(),
                        ..FlatModel::default()
                    },
                    FlatModel {
                        name: Some("b".to_string()),
                        type_: "number".to_string(),
                        attributes: Attributes {
                            required: false,
                            ..Attributes::default()
                        },
                        ..FlatModel::default()
                    }
                ]
            }))
        );
    }

    #[test]
    fn test_should_wrap_optional_and_nullable() {
        let schema = json!({
            "title": "MySchema",
            "required": ["property1"],
            "properties": {
                "property1": { "type": "string" },
                "property2": { "type": "number", "nullable": true }
            }
        });

        let mut container = ModelContainer::default();
        let mut scope = SchemaScope::default();
        let resolver = SchemaResolver::empty();
        let mut options = JsonSchemaExtractOptions::default();
        options.optional_and_nullable_as_models = true;

        scope.entity("TestName");

        let result = from_object_with_properties(
            schema.as_object().unwrap(),
            &mut container,
            &mut scope,
            &resolver,
            &options,
        );

        assert_eq!(
            result.unwrap(),
            Model::new(ModelType::ObjectType(ObjectType {
                name: "MySchema".to_string(),
                properties: vec![
                    FlatModel {
                        name: Some("property1".to_string()),
                        type_: "string".to_string(),
                        attributes: Attributes {
                            required: true,
                            ..Attributes::default()
                        },
                        ..FlatModel::default()
                    },
                    FlatModel {
                        name: Some("property2".to_string()),
                        type_: "wrapper".to_string(),
                        model: Some(Box::new(FlatModel {
                            name: Some("TestNameProperty2Optional".to_string()),
                            type_: "number".to_string(),
                            ..FlatModel::default()
                        })),
                        original: Some(0),
                        ..FlatModel::default()
                    }
                ],
            }))
        );

        scope.form("properties");
        scope.property("property2");
        let wrapper = container.resolve(&mut scope).unwrap();
        scope.reduce(2);

        assert_eq!(
            wrapper,
            &Model::new(ModelType::NullableOptionalWrapperType(
                NullableOptionalWrapperType {
                    name: "TestNameProperty2Optional".to_string(),
                    model: FlatModel {
                        name: Some("property2".to_string()),
                        type_: "number".to_string(),
                        ..FlatModel::default()
                    }
                }
            ))
        );
    }
}
