namespace a {
int f(int x) { return x; }
struct S { int f; };
int u() {
  S s{1};
  s.f = f(2);
  return s.f;
}
}
