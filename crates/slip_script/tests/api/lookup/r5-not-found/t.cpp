template <class T>
struct C {
  T t;
};
template <>
struct C<int> {
  int s;
};
template <class T>
struct C<T*> {
  int p;
};
template struct C<long>;
template <class T>
int v = 0;
template <>
int v<int> = 1;
int f() {
  int local = 0;
  struct L {
    int k;
  };
  return local;
}
