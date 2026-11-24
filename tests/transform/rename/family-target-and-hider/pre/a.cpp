namespace n {
struct A { int f(int x) { return x; } };
struct B : A { virtual int f() { return 0; } };
struct D : B {
  int f() override { return 1; }
  using B::f;
};
}
int u(n::D &d) { return d.f() + d.A::f(2); }
