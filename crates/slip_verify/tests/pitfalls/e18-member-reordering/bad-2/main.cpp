struct B {
  virtual int g();
  virtual int f();
  int h();
  int k();
};
int B::f() { return 1; }
int B::g() { return 2; }
int B::h() { return 3; }
int B::k() { return 4; }

int call(B &b) { return b.f() + b.g() + b.h() + b.k(); }
