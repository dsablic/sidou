// SPDX-FileCopyrightText: 1998-2004 Dag Lem <resid@nimrod.no>
// SPDX-License-Identifier: GPL-2.0-or-later
/* Filter cutoff measurements (FC register -> Hz) from reSID 0.16 filter.cc,
 * with its duplicated spline end points removed. */
#ifndef SIDOU_SID_CURVES_RESID016_H
#define SIDOU_SID_CURVES_RESID016_H

static const float SID_F0_POINTS_6581[][2] = {
    {0, 220},     {128, 230},   {256, 250},   {384, 300},   {512, 420},   {640, 780},
    {768, 1600},  {832, 2300},  {896, 3200},  {960, 4300},  {992, 5000},  {1008, 5400},
    {1016, 5700}, {1023, 6000}, {1024, 4600}, {1032, 4800}, {1056, 5300}, {1088, 6000},
    {1120, 6600}, {1152, 7200}, {1280, 9500}, {1408, 12000}, {1536, 14500}, {1664, 16000},
    {1792, 17100}, {1920, 17700}, {2047, 18000}};

static const float SID_F0_POINTS_8580[][2] = {
    {0, 0},       {128, 800},   {256, 1600},  {384, 2500},  {512, 3300},  {640, 4100},
    {768, 4800},  {896, 5600},  {1024, 6500}, {1152, 7500}, {1280, 8400}, {1408, 9200},
    {1536, 9800}, {1664, 10500}, {1792, 11000}, {1920, 11700}, {2047, 12500}};

#endif
