namespace n {
struct B { int f() { return 0; } };
struct D : B {
  using B::f;
  int f() { return 1; }
};
}
int u(n::D &d) { return d.f(); }
