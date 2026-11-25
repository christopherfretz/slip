namespace a {
int f(int x) { return x; }
namespace in { int f(int x) { return x + 1; } }
struct C { int f(int x) { return x + 2; } };
}
namespace b { int f(int x) { return x + 3; } }
int u(a::C c) { return a::f(1) + a::in::f(2) + c.f(3) + b::f(4); }
