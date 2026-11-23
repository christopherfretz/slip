int g(long) { return 1; }
int f(int) { return 2; }

int caller() {
  return g(1) + f(2);
}
