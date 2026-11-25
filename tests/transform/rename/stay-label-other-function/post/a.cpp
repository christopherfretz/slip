namespace a {
int g(int x) { return x; }
int use() { return g(1); }
int other(int n) {
  int r = 0;
f:
  if (r < n) {
    ++r;
    goto f;
  }
  return r;
}
}
