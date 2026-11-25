namespace a {
void f(int *p) { *p = 0; }
int u() {
  int x __attribute__((cleanup(f))) = 1;
  return x;
}
}
