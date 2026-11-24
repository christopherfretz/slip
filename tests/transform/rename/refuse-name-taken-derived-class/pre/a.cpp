namespace a {
struct B { int f() { return 1; } };
struct D : B { int g() { return 2; } };
}
int u(a::D &d) { return d.f() + d.g(); }
