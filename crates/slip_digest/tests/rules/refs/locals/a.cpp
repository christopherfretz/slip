namespace acme {
  int f(int n) {
    struct L { int k; };
    static int count = 0;
    auto lam = [n](int m) { struct In { int q; }; return In{m}.q + n; };
    L l{n};
    return lam(l.k) + ++count;
  }
  template<class T> T twice(T t) { return t + t; }
}
int user() { return acme::f(2) + acme::twice(3) + acme::twice(4L); }
