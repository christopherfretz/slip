namespace a {
int g(int x) { return x; }
int u(int n) {
  int r = 0;
f:
  r += g(1);
  if (r < n)
    goto f;
  return r;
}
}
