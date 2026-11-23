struct M {
  int k;
  M(int k) : k(k) {}
  M(const M &) : k(1) {}
  M(M &&) : k(2) {}
};

M pass(M &m) { return m; }
int kind() { M m(0); return pass(m).k; }
