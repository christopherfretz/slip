#include "side.h"
// digest §3.4, WP-039 item 1: conversion functions: two keys.
struct S { operator int(); };
#ifdef B
struct S2 { operator int(); }; struct S3 { operator long(); };
#endif
