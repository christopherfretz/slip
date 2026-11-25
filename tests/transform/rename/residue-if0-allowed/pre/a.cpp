namespace a { int f(int x) { return x; } }
int u() {
#if 0
  return a::f(0);
#endif
  return a::f(1);
}
