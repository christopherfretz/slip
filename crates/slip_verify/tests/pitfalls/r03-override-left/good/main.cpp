struct B {
  virtual int g() { return 0; }
};
struct D1 : B {
  int g() { return 1; }
};
struct D2 : B {
  int g() { return 2; }
};

int call(B &b) { return b.g(); }
