// §10.1, the replacement `lib::bar` is not declared: each call reported.
namespace lib {
int foo(int a, int b);
}
int use() { return lib::foo(1, 2); }
