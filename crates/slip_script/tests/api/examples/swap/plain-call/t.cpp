// spec/api.md §10.1, a plain call `lib::foo(x, y)`: the callee is respelled
// (the sibling spelling, §7.2 rule 1) and the arguments exchanged, as one
// group; the enclosing function is declared changed.
namespace lib {
int foo(int a, int b);
int bar(int a, int b);
}
int use(int x, int y) { return lib::foo(x, y); }
