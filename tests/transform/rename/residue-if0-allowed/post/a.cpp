namespace a { int g(int x) { return x; } }
int u() {
#if 0
  return a::f(0);
#endif
  return a::g(1);
}
