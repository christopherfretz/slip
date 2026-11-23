struct Base {
  virtual int f() { return 1; }
  int run() { return this->f(); }
};

struct Derived : Base {
  int f() override { return 2; }
};
