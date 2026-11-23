int f(int) { return 1; }
int f(unsigned) { return 2; }

int caller() {
  auto x = 1u;
  return f(x);
}
