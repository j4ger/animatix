//! Source-order argument placement shared by callable implementations.

/// The argument-binding portion of either a source or catalog parameter.
#[derive(Clone, Copy, Debug)]
pub struct ArgumentParameter<'a> {
    /// Name accepted by named arguments.
    pub name: &'a str,
    /// Collect trailing positional arguments; must be the final parameter.
    pub variadic: bool,
}

/// Arguments placed into one parameter, before defaults and type validation.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgumentSlot<T> {
    /// An ordinary parameter, possibly omitted.
    Single(Option<T>),
    /// Elements supplied to a variadic parameter.
    Variadic(Vec<T>),
}

/// Place positional and named arguments without evaluating defaults or values.
///
/// Keeping these rules independent of the value representation lets source
/// functions, standard functions and native extensions use identical binding.
pub fn bind_arguments<T>(
    callable: &str,
    parameters: &[ArgumentParameter<'_>],
    arguments: impl IntoIterator<Item = (Option<String>, T)>,
) -> Result<Vec<ArgumentSlot<T>>, String> {
    let mut slots = parameters
        .iter()
        .map(|parameter| {
            if parameter.variadic {
                ArgumentSlot::Variadic(Vec::new())
            } else {
                ArgumentSlot::Single(None)
            }
        })
        .collect::<Vec<_>>();
    let mut positional = 0;
    let mut named_seen = false;
    for (name, value) in arguments {
        if let Some(name) = name {
            named_seen = true;
            let index = parameters
                .iter()
                .position(|parameter| parameter.name == name)
                .ok_or_else(|| format!("call to '{callable}' has no parameter named '{name}'"))?;
            match &mut slots[index] {
                ArgumentSlot::Single(slot) => {
                    if slot.is_some() {
                        return Err(format!(
                            "parameter '{name}' of '{callable}' was supplied more than once"
                        ));
                    }
                    *slot = Some(value);
                },
                ArgumentSlot::Variadic(_) => {
                    return Err(format!(
                        "variadic parameter '{name}' of '{callable}' cannot be named"
                    ));
                },
            }
        } else {
            if named_seen {
                return Err(format!(
                    "positional arguments to '{callable}' must come before named arguments"
                ));
            }
            let slot = slots
                .get_mut(positional)
                .ok_or_else(|| format!("call to '{callable}' has too many positional arguments"))?;
            match slot {
                ArgumentSlot::Single(slot) => {
                    *slot = Some(value);
                    positional += 1;
                },
                ArgumentSlot::Variadic(items) => items.push(value),
            }
        }
    }
    Ok(slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_names_and_retains_omissions_for_definition_defaults() {
        let parameters = ["first", "second", "third"].map(|name| ArgumentParameter {
            name,
            variadic: false,
        });
        assert_eq!(
            bind_arguments("sample", &parameters, [(None, 1), (Some("third".into()), 3)]),
            Ok(vec![
                ArgumentSlot::Single(Some(1)),
                ArgumentSlot::Single(None),
                ArgumentSlot::Single(Some(3)),
            ])
        );
        for arguments in [
            vec![(None, 1), (Some("first".into()), 2)],
            vec![(Some("second".into()), 2), (None, 1)],
            vec![(Some("missing".into()), 1)],
            vec![(None, 1), (None, 2), (None, 3), (None, 4)],
        ] {
            assert!(bind_arguments("sample", &parameters, arguments).is_err());
        }
    }

    #[test]
    fn variadics_collect_only_trailing_positional_values() {
        let parameters = [
            ArgumentParameter {
                name: "format",
                variadic: false,
            },
            ArgumentParameter {
                name: "values",
                variadic: true,
            },
        ];
        assert_eq!(
            bind_arguments("format", &parameters, [(None, 1), (None, 2), (None, 3)]),
            Ok(vec![
                ArgumentSlot::Single(Some(1)),
                ArgumentSlot::Variadic(vec![2, 3])
            ])
        );
        assert!(bind_arguments("format", &parameters, [(Some("values".into()), 1)]).is_err());
    }
}
