template <class T> struct traits {
  using type = int;
};
template <class T> long tag(T) { return 1; }
template <class T> struct box {
  typename traits<T>::type width;
  decltype(tag(T())) found;
};
