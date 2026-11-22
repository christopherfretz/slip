// §10.7 (WP-033 review, item 7): a line that starts with `module` (here a variable's name) makes Kiln track ownership (kiln_pp/src/modules.rs:403); nothing changes.
int module; void f() { module = 1; } int g; struct S { int m; };
