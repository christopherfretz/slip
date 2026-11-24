namespace a {
int f(int x) { return x; }
int u(int n) {
  int r = 0;
f:
  r += f(1);
  if (r < n)
    goto f;
  return r;
}
}
