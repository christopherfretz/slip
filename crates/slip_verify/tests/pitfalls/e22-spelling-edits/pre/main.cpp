struct S {
  int v;
  S() {}
};

int trivial() { return __is_trivially_constructible(S); }
