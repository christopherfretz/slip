namespace a {
struct C { int n(int x) { return x; } };
}
int (a::C::*pm)(int) = &a::C::n;
int u(a::C &c) { return (c.*pm)(1); }
namespace a {
auto q = &C::n;
}
