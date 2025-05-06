# match-template

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![Apache 2.0 licensed][license-badge]][license-url]

[crates-badge]: https://img.shields.io/crates/v/match-template.svg
[crates-url]: https://crates.io/crates/match-template
[docs-badge]: https://docs.rs/match-template/badge.svg
[docs-url]: https://docs.rs/match-template
[license-badge]: https://img.shields.io/crates/l/match-template
[license-url]: LICENSE

## Overview

match-template is a procedural macro that generates repeated match arms by pattern.

This crate provides a macro that can be used to append a match expression with multiple arms, where the tokens in the first arm, as a template, can be substituted and the template arm will be expanded into multiple arms.

For example, the following code

```rust
match_template! {
    T = [Int, Real, Double],
    match Foo {
        EvalType::T => { panic!("{}", EvalType::T); },
        EvalType::Other => unreachable!(),
    }
}
```

generates

```rust
match Foo {
    EvalType::Int => { panic!("{}", EvalType::Int); },
    EvalType::Real => { panic!("{}", EvalType::Real); },
    EvalType::Double => { panic!("{}", EvalType::Double); },
    EvalType::Other => unreachable!(),
}
```

In addition, substitution can vary on two sides of the arms.

For example, the following code

```rust
match_template! {
    T = [Foo, Bar => Baz],
    match Foo {
        EvalType::T => { panic!("{}", EvalType::T); },
    }
}
```

generates

```rust
match Foo {
    EvalType::Foo => { panic!("{}", EvalType::Foo); },
    EvalType::Bar => { panic!("{}", EvalType::Baz); },
}
```

Wildcard match arm is also supported (but there will be no substitution).

## License and Origins

This project is licensed under the Apache License, Version 2.0. See the [LICENSE](LICENSE) file for details.

This repository is a fork of [tikv/match-template](https://github.com/tikv/match-template) since the upstream is unmaintained. The original project is licensed under the Apache License, Version 2.0.
