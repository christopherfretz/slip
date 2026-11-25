namespace n {
struct B { int g() { return 0; } };
struct D : B {
  using B::g;
  int f(int x) { return x; }
};
}
int u(n::D &d) { return d.g() + d.f(1); }
