Please view `crates/vue_oxlint_parser`, it is a Vue 3 parser, which can generate a AST that compatible with `vue-eslint-parser`.

Most of the AST node types and tokens types have been defined, including serialization for downstream / napi compatibility. But the core logic (including lexer / parser logic is not finished) please finish it in this goal.

## Requirement

- Strictly follow lexer / parser and recursive decent parser architecture, write lexing core logic in `src/lexer`, core parser logic (base on the token stream, not directly base on the chars or source_text(you can read the source_text though)) in `src/parser/parse`.
- Use ast defined in src/ast directory, construct ast while in parsing logic.
- Use token stream to control parsing process. When meeting unrecoverable error, let the lexer jump to eof and return the result with `panicked: true` field.
- `vue-eslint-parser` needs token output, I've already finished tokens serialization logic and added fields in the return sfc, please follow the comments' requirement.
- Remove all `#[allow(dead_code)]` and `todo!()`, make `VueParser::parse` return the true ast.
- Do not modify code under `crates/vue_oxlint_jsx` crate, it will get migrated according to `AGENTS.md` in the future but not now.
- Lexer should have high accuracy, which means it can handle `v-pre`, `<script lang="ts"> defineProps<SimilarToTagButNot>() </script>` etc.
- Error messages (diagnostics) should be defined unitedly in src/error.rs`. They should be roughly the same as `vue-eslint-parser`.
- We should also implement some logic in `vue-oxlint-toolkit` (napi/js-side), use estree trait (json) and JSON.parse to pass the ast from rust to js side, implement a transforming logic (moving script nodes into body, and move tokens, comments logic, add `loc` getter hook, add `parent` pointer), the traverse logic maybe need visitor keys, which make our VueSingleFileComponent struct to be in the same type as `vue-eslint-parser`'s output for compatibility with `eslint`. Implement `parse` function defined in the `index.ts` as well (logic should be moved into other files) (for now, just keep `null` for `transform` field, do not call `transformJsx`).

## Tips

- For script parsing, or some directives (like v-for, v-slot handling) processing, perhaps the code in `vue_oxlint_jsx/src/parser/elements` can be used as a reference.
- For tests, you should not add any ast related tests in the `vue_oxlint_parser` crate, it should all in the `vue-oxlint-toolkit` crate as napi tests, using vitest's `expect(parse(....)).toEqual(../* vue-eslint-parser's output with the same as */.)` Use project global fixtures (use for to traverse these files, do not run that test for error / panic files). There are no tests planned specifically for lexer / parser. The test only worth adding is the result's metadata test, like irregular_whitespaces and module_records. You can copy them directly from `vue_oxlint_jsx/src`
- I suggest writing out the project skeleton first, such as the methods we should call. You can use `todo!()` temporarily and implement them later.

There are some code bases I've cloned, feel free to use / add experiment files inside their codebase if you want to get more hints and do some experiments

- Oxc: `~/code/oxc-project/oxc`
- Vue Eslint Parser: `~/code/vuejs/vue-eslint-parser`
- Vue EsLint Parser Demo `~/code/liang-demos/vue-eslint-parser-demo`, you can view the token struct and ast struct by modifying main.ts and use `pnpm run r` to print things you want.

## Some problems worth mentioning

# Tokens Output

- `vue-eslint-parser`'s lexer is strange, it will produces HTMLWhitespace token when meet whitespace characters no matter where, even in raw mode script, means HTMLRawText and HTMLWhitespace can be interspersed and kept in `ret.templateBody.tokens` in the end. But it also has replacing logic, like when meet `{{ a + 1 }}` in template, they will produces HTMLText and HTMLWhitespace first, but they will be replaced to standard script tokens when parsing script (means no HTMLWhitespace token here). That's behavior is quite weird. (This is a really complex one, if you get confused, you can try some demos in `~/code/liang-demos/vue-eslint-parser-demo`) We should follow what `vue-eslint-parser` does in `<script>` tag, produces HTMLWhitespace and HTMLRawText, let parser consume them together, but for the scripts in template, we should produce a long HTMLText node, and replace it to standard script tokens(generate by oxc_parse func) later for better performance. We may can also record irregular_whitespaces the lexer meets and use lexer's irregular_whitespaces instead of a separated string traverse to improve parser performance.
- `vue-eslint-parser` produces two collection of tokens, one is `ret.tokens` which contains only script tokens, another is `ret.templateBody.tokens` which contains tokens of the whole SFC, we also have two token definition in the returned result, ret.tokens include `<script>` as punctuators and js tokens in <script> tags, but template tokens includes the HTMLRawText and HTMLWhitespace for <script> tag, just based on the lexer's tokens output.
- The tokens array printing is already implemented, but not tested, perhaps it has some small problems like `,` and small length (+1, -1) problems. Please correct errors if you find them, but please retain the predefined string pre-storage + `print_str` to buffer structure I have already defined.

# Reference and Variable

- `vue-eslint-parser` requires `references` field for every `VExpressionContainer` to trace variable references and `variable` field for every `VStartTag` to trace the variable creates in this tag. It requires parser to traverse the generated script ast nodes, find all references and binding creations (binding creation should only happens on v-slot / v-for tag? so maybe they are in the FormalParameters) and set the field. We can't use `oxc_ast_visit`, as they perhaps have complex situation `v-for="(a, b) in ((x)=>{return x+1})(y)"`, we actually only reference `y` here, not x, but there are exactly has `x` as `IdentifierReference`, we have to import `oxc_semantic` crate here. Do not care about generic for now.

## Rules / Code Style

- Prefer dividing code into multiple files to keep the code clean and maintainable (like what I did in ast definition files). Especially when you are writing the core logic about lexer / parser and doing transform work in `src/transform` directory.
- `unwrap`, `unreachable!()` and other panicking methods are allowed and recommended to simplify code. `unsafe` is allowed and recommended to improve performance. But you need to add comments `// SAFETY: ` to explain why it won't panic / cause UB actually.

Read my code-style skills to learn more.

## Your Implementation Roadmap (Please finish all of them as your goal)

Phase 1: implement lexer
Phase 2: implement parser (the hardest part, may need a lot of time, plan and use your `todo!()` wisely)
Phase 3: Add transform in toolkit (js side) (traverse, add missing fields)
Phase 4: Add AST tests in toolkit (Vitest JS side)
Phase 5: Fix compatibility bugs (tokens printing, token array lengths, etc.) and improve performance.

Please maintain a `TODO.md` in the project root to record the things that need to be implemented, and problems you meet, and thoughts / workaround you did.

Do not add `PROMPT.md` and `TODO.md` to git.
