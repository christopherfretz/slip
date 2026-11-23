// §10.1, a call of `lib::foo` nested in an argument of another: both are
// rewritten; the inner call's edits lie inside the outer call's first
// argument, which the outer call copies (§7.1 rule 3).
namespace lib {
int foo(int a, int b);
int bar(int a, int b);
}
int use() { return lib::foo(lib::foo(1, 2), 3); }
