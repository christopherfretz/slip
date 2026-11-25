namespace a {
struct B { int g() { return 1; } };
struct D : B {
  int f() { return 2; }
  int k() { return g() + f(); }
};
}
