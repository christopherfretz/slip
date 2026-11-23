namespace n { void f(int); }
struct S { int f; };
int h(int f) {
  int g2 = f;
  { int f = 2; g2 += f; }
  S s{1};
  g2 += s.f;
f:
  if (g2 > 100) goto f;
  n::f(g2);
  return g2;
}
