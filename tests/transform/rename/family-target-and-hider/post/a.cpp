namespace n {
struct A { int f(int x) { return x; } };
struct B : A { virtual int g() { return 0; } };
struct D : B {
  int g() override { return 1; }
  using B::g;
};
}
int u(n::D &d) { return d.g() + d.A::f(2); }
