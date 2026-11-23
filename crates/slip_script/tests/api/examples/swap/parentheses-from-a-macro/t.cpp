// §10.1, a call whose parentheses and arguments come from a macro, the
// callee from its argument: the arguments have no range; reported.
#define APPLY(f) f(1, 2)
namespace lib {
int foo(int a, int b);
int bar(int a, int b);
}
int use() { return APPLY(lib::foo); }
