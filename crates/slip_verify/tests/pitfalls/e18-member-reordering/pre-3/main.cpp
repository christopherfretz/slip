struct S {
  int a;
  int b;
  S(int v) : a(v), b(a + 1) {}
};

int second() { return S(1).b; }
