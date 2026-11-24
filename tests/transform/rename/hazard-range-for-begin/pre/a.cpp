namespace a {
struct C {
  int d[2] = {1, 2};
  int *begin() { return d; }
  int *end() { return d + 2; }
};
}
int u() {
  a::C c;
  int s = 0;
  for (int x : c)
    s += x;
  return s + *c.begin();
}
