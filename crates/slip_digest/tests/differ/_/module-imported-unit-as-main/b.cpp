// §10.7 "in the Ast like its own": the declarations of `m` reached through `import m;` (side A) digest as when `m` is the main file (side B). Only the order unit differs, by the import.
export module m;
export int f();
int g();
export struct S { int a; };
