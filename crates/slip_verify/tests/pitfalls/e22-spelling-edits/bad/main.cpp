struct S {
  int v;
  S() = default;
};

int trivial() { return __is_trivially_constructible(S); }
