struct S {
  int a;
  int b;
  S(int v) : b(a + 1), a(v) {}
};

int second() { return S(1).b; }
