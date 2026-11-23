// §10.1, calls with one and with three written arguments (a default
// argument and an ellipsis): each reported for review, none rewritten.
namespace lib {
int foo(int a, int b = 0, ...);
int bar(int a, int b);
}
int use() { return lib::foo(1) + lib::foo(1, 2, 3); }
