// §10.1, a fully qualified call `::lib::foo(1, 2)` and a call `foo(3, 4)`
// written inside the namespace: both rewritten, each its own group.
namespace lib {
int foo(int a, int b);
int bar(int a, int b);
}
int use() { return ::lib::foo(1, 2); }
namespace lib {
int in() { return foo(3, 4); }
}
