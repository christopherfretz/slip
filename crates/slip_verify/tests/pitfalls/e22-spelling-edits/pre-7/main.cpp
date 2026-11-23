struct M {
  int k;
  M(int k) : k(k) {}
  M(const M &) : k(1) {}
  M(M &&) : k(2) {}
};

M make() {
  M m(0);
  return m;
}
int kind() { return make().k; }
