namespace a {
int f(int x) { return x; }
int u(int n) {
  void *p = &&f;
  int r = 0;
f:
  r += f(1);
  if (r < n)
    goto *p;
  return r;
}
}
