// §10.1, no file of the subject is owned (`// owned: no` in the
// transform): the call is reported, not rewritten.
namespace lib {
int foo(int a, int b);
int bar(int a, int b);
}
int use() { return lib::foo(1, 2); }
