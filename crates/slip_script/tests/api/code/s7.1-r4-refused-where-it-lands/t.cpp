template <class T> T tf(T);
template <class T>
struct W {
  static int g();
};
template <>
struct W<char> {
  static int g();
};
struct A {
  union {
    int u;
  };
};
struct Fr {
  friend int only_friend(Fr);
};
int take(int, int, int, int, int, int);
int probe(int);
int use(W<int> w, W<char> c, A a, Fr fr) {
  return tf(1) + w.g() + probe(c.g()) + a.u + only_friend(fr) +
         take(1, 2, 3, 4, 5, 6);
}
