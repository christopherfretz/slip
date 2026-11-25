namespace a {
int g(int x) { return x; }
int u(int n) {
  void *p = &&f;
  int r = 0;
f:
  r += g(1);
  if (r < n)
    goto *p;
  return r;
}
}
