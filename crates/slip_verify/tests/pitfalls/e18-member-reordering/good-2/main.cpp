struct B {
  virtual int f();
  virtual int g();
  int k();
  int h();
};
int B::f() { return 1; }
int B::g() { return 2; }
int B::h() { return 3; }
int B::k() { return 4; }

int call(B &b) { return b.f() + b.g() + b.h() + b.k(); }
