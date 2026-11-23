struct Base {
  int f(int) { return 1; }
  int f(const char *) { return 3; }
};
struct Derived : Base {
  using Base::f;
  int f(long) { return 2; }
};

int caller() { Derived d; return d.f(1); }
