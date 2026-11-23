namespace legacy { template <class T> struct W { T t; }; }
template <template <class> class TT> struct H { TT<int> x; };
H<legacy::W> h;
int use() { return h.x.t; }
