struct S {
  int b;
  int a;
  S(int v) : a(v), b(a + 1) {}
};

int second() { return S(1).b; }
