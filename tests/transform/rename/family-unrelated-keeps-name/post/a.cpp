namespace n {
struct B { virtual int g() { return 0; } };
struct D : B { int g() override { return 1; } };
struct U { virtual int f() { return 9; } };
}
int u(n::B &b, n::U &x) { return b.g() + x.f(); }
