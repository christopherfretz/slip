template <class T> struct traits {
  using type = int;
};
template <class T> int width() { return sizeof(typename traits<T>::type); }
