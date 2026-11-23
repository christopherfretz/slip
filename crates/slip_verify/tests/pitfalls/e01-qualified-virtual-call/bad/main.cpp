struct Base {
  virtual int f() { return 1; }
  int run() { return Base::f(); }
};

struct Derived : Base {
  int f() override { return 2; }
};
