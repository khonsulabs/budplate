# budplate

A template engine built around the [Bud][budlang] language.

**This repository is currently in an experimental phase.**

Budplate scans a template looking for regions between `{{` and `}}`. For
example, this template allows a parameter `name` to be substituted when
rendered:

```budplate
Hello, {{= name }}!
```

And, to render the template in Rust:

```rust
use budplate::Template;

let rendered = Template::from("Hello, {{= name }}!")
                    .render_with([("name", "World")]).unwrap();
assert_eq!(rendered, "Hello, World!");
```

## Inline Expressions

Inline expressions use the `{{= expression }}` syntax. By default, expressions
are automatically encoded. For example, when rendering a template with
`HtmlEncoding`, all expressions will automatically be encoded to ensure text
does not interfere with markup:

```rust
use budplate::Configuration;

let rendered = Configuration::for_html()
                    .render_with(
                        "Hello, {{= name }}!", 
                        [("name", "Robert </table>")]
                    ).unwrap();
assert_eq!(rendered, "Hello, Robert &lt;/table&gt;!");
```

If you wish to avoid encoding an inline expression, use the `{{:= expression }}`
syntax:

```rust
use budplate::Configuration;

let rendered = Configuration::for_html()
                    .render_with(
                        "Hello, {{:= name }}!", 
                        [("name", "Robert </table>")]
                    ).unwrap();
assert_eq!(rendered, "Hello, Robert </table>!");
```

## Statements

To support logic such as loops and if statements, individual Bud statements can
be added using the `{{ statement }}` syntax. Statements do not directly affect
the rendered output, but are still useful for control flow:

```rust
use budplate::Template;

let rendered = Template::from(
                   "Easy as {{ loop for i := 1 to 3 inclusive }}{{= i }}{{ end }}")
               .render().unwrap();
assert_eq!(rendered, "Easy as 123");
```

## Whitespace trimming

Whitespace can be automatically trimmed around all template directives. By
adding a `-` at the start, Budplate will automatically trim any preceding
whitespace before the template directive. For example:

- To trim before:

  ```budplate
  {{=- expression }}
  {{:=- expression }}
  {{- statement }}
  ```

  ```rust
  use budplate::Template;
  
  let rendered = Template::from("( {{=- 1 }} )")
                      .render().unwrap();
  assert_eq!(rendered, "(1 )");
  ```

- To trim after:

  ```budplate
  {{= expression -}}
  {{:= expression -}}
  {{ statement -}}
  ```

  ```rust
  use budplate::Template;
  
  let rendered = Template::from("( {{= 1 -}} )")
                      .render().unwrap();
  assert_eq!(rendered, "( 1)");
  ```

- To trim before and after:

  ```budplate
  {{=- expression -}}
  {{:=- expression -}}
  {{- statement -}}
  ```

  ```rust
  use budplate::Template;
  
  let rendered = Template::from("( {{=- 1 -}} )")
                      .render().unwrap();
  assert_eq!(rendered, "(1)");
  ```

[budlang]: https://github.com/khonsulabs/budlang
