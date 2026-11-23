struct B {
  virtual int f() { return 0; }
};
struct D1 : B {
  int f() { return 1; }
};
struct D2 : B {
  int f() { return 2; }
};

int call(B &b) { return b.f(); }
