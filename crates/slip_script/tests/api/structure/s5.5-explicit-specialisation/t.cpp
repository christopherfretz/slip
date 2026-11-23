template <class T>
void tf(T) {}
template <>
void tf<int>(int) {}
template <class T>
struct S {
  void g() {}
};
template <>
void S<char>::g() {}
template <class T>
int vt = 1;
template <>
int vt<int> = 2;
int use() {
  tf(1);
  S<char>().g();
  return vt<int>;
}
