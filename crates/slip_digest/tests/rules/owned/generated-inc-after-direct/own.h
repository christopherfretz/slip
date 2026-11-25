#ifndef OWN_H
#define OWN_H
namespace llvm {
#ifdef POST
bool parseAsU(unsigned long long &R);
inline bool use(unsigned long long &R) { return parseAsU(R); }
#else
bool getAsU(unsigned long long &R);
inline bool use(unsigned long long &R) { return getAsU(R); }
#endif
}
#endif
