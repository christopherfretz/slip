// §10.1, the function named and not called (`&lib::foo`): reported for
// review; the call beside it is rewritten.
namespace lib {
int foo(int a, int b);
int bar(int a, int b);
}
int (*p)(int, int) = &lib::foo;
int use() { return lib::foo(1, 2); }
