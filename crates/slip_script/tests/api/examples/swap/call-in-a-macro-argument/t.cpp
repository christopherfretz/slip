// §10.1, a call inside a macro argument: tier 1 (§5.4), editable,
// rewritten at tier 1.
#define ID(x) (x)
namespace lib {
int foo(int a, int b);
int bar(int a, int b);
}
int use() { return ID(lib::foo(1, 2)); }
