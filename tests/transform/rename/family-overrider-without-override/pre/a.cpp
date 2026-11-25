namespace n {
struct B { virtual int f() { return 0; } };
struct D : B { int f() { return 1; } };
struct E : D { virtual int f() { return 2; } };
}
int u(n::E &e) { return e.f(); }
