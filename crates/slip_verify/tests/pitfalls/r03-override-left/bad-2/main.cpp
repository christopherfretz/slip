struct B {
  virtual int g() { return 0; }
};
struct D : B {
  int g() { return 1; }
};

int call(B &b) { return b.g(); }
