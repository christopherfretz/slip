// §10.1, a call that a macro's replacement list writes: its extent is the
// invocation, its callee has none; reported for review.
#define CALL lib::foo(1, 2)
namespace lib {
int foo(int a, int b);
int bar(int a, int b);
}
int use() { return CALL; }
