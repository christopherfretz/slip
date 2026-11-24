template <class T> T tf(T);
template <class T> struct S {
  int m;
  static int k();
};
int use_tf = tf(1);
int use_k = S<int>::k();
struct P {
  int f(int p);
};
namespace {
int in_unnamed;
}
struct A {
  union {
    int u;
  };
};
struct Fr {
  friend void only_friend(Fr);
};
