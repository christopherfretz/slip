// §10.1, the same spelling `foo` for other entities (`other::foo`, also
// through a using-directive, and a variable `fourth::foo`): untouched; only
// `lib::foo(5, 6)` is rewritten.
namespace lib {
int foo(int a, int b);
int bar(int a, int b);
}
namespace other {
int foo(int a, int b);
}
namespace fourth {
int foo;
}
namespace third {
using namespace other;
int v = foo(3, 4);
}
int use() { return other::foo(1, 2) + fourth::foo + lib::foo(5, 6); }
