namespace n { template <class T> struct S { template <class U> void g() {} };
template <> template <class U> void S<int>::g() {} }
void use() { n::S<int>().g<long>(); n::S<char>().g<long>(); }
