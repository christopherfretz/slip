namespace a {
void g(int *p) { *p = 0; }
int u() {
  int x __attribute__((cleanup(g))) = 1;
  return x;
}
}
