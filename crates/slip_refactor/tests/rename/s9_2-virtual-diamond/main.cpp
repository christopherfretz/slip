namespace n {
struct B { virtual int f(); };
struct L : virtual B { int f() override; };
struct R : virtual B { int f() override; };
struct D : L, R { int f() override; };
}
int h(n::D& d) { return d.f(); }
