template <class T> struct Box {
  T v;
  Box(T x);
};
Box(int) -> Box<long>;
template <class T> using Alias = Box<T>;

int caller() { Alias a{1}; return sizeof(a.v); }
