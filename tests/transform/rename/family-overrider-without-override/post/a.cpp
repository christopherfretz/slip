namespace n {
struct B { virtual int g() { return 0; } };
struct D : B { int g() { return 1; } };
struct E : D { virtual int g() { return 2; } };
}
int u(n::E &e) { return e.g(); }
