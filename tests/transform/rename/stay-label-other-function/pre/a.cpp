namespace a {
int f(int x) { return x; }
int use() { return f(1); }
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
