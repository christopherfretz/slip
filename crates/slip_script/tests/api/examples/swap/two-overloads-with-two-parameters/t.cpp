// §10.1, two functions with two parameters named `lib::foo`: an error
// report with no place, nothing rewritten.
namespace lib {
int foo(int a, int b);
int foo(double a, double b);
int bar(int a, int b);
}
int use() { return lib::foo(1, 2); }
