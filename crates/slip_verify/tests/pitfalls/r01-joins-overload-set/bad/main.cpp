int g(long) { return 1; }
int g(int) { return 2; }

int caller() {
  return g(1) + g(2);
}
