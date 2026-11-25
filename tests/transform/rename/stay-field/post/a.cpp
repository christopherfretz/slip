namespace a {
int g(int x) { return x; }
struct S { int f; };
int u() {
  S s{1};
  s.f = g(2);
  return s.f;
}
}
