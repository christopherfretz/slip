namespace n {
struct B { int f() { return 0; } };
struct D : B {
  using B::f;
  int f(int x) { return x; }
};
}
int u(n::D &d) { return d.f() + d.f(1); }
