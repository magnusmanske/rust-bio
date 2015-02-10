// Copyright 2014 Johannes Köster.
// Licensed under the MIT license (http://opensource.org/licenses/MIT)
// This file may not be copied, modified, or distributed
// except according to those terms.

//! The Ferragina-Mancini Index for finding suffix array intervals matching a given pattern.

use std::iter::DoubleEndedIterator;

use data_structures::bwt::{Occ, Less, less, BWT};
use data_structures::suffix_array::SuffixArray;
use alphabets::{Alphabet, dna};
use std::mem::swap;


pub struct FMIndex<'a> {
    bwt: &'a BWT,
    less: Less,
    occ: Occ
}


impl<'a> FMIndex<'a> {
    /// Construct a new instance of the FM index.
    ///
    /// # Arguments
    ///
    /// * `bwt` - the BWT
    /// * `k` - the sampling rate of the occ array: every k-th entry will be stored (higher k means
    ///   less memory usage, but worse performance)
    /// * `alphabet` - the alphabet of the underlying text, omitting the sentinel
    pub fn new(bwt: &'a BWT, k: usize, alphabet: &Alphabet) -> Self {
        FMIndex { bwt: bwt, less: less(bwt, alphabet), occ: Occ::new(bwt, k, alphabet)}
    }

    /// Perform backward search, yielding suffix array
    /// interval denoting exact occurences of the given pattern of length m in the text.
    /// Complexity: O(m).
    ///
    /// # Arguments
    ///
    /// * `pattern` - the pattern to search
    ///
    /// # Example
    ///
    /// ```
    /// use bio::data_structures::bwt::bwt;
    /// use bio::data_structures::fmindex::FMIndex;
    /// use bio::data_structures::suffix_array::suffix_array;
    /// use bio::alphabets::dna;
    /// let text = b"GCCTTAACATTATTACGCCTA$";
    /// let alphabet = dna::alphabet();
    /// let pos = suffix_array(text);
    /// let bwt = bwt(text, &pos);
    /// let fm = FMIndex::new(&bwt, 3, &alphabet);
    /// let pattern = b"TTA";
    /// let sai = fm.backward_search(pattern.iter());
    /// assert_eq!(sai, (19, 21));
    /// ```
    pub fn backward_search<'b, P: Iterator<Item=&'b u8> + DoubleEndedIterator>(&self, pattern: P) -> (usize, usize) {
        let (mut l, mut r) = (0, self.bwt.len() - 1);
        for &a in pattern.rev() {
            let less = self.less(a);
            l = less + if l > 0 { self.occ(l - 1, a) } else { 0 };
            r = less + self.occ(r, a) - 1;
        }

        (l, r)
    }

    fn occ(&self, r: usize, a: u8) -> usize {
        self.occ.get(self.bwt, r, a)
    }

    fn less(&self, a: u8) -> usize {
        self.less[a as usize]
    }
}


#[derive(Copy)]
#[derive(Debug)]
pub struct BiInterval {
    lower: usize,
    lower_rev: usize,
    size: usize,
    match_size: usize,
}


impl BiInterval {
    pub fn occ<'a>(&self, pos: &'a SuffixArray) -> &'a [usize] {
        self._pos(pos, self.lower)
    }

    pub fn occ_revcomp<'a>(&self, pos: &'a SuffixArray) -> &'a [usize] {
        self._pos(pos, self.lower_rev)
    }

    fn _pos<'a>(&self, pos: &'a SuffixArray, lower: usize) -> &'a [usize] {
        &pos[lower..lower + self.size]
    }

    fn swapped(&self) -> BiInterval {
        BiInterval {
            lower: self.lower_rev,
            lower_rev: self.lower,
            size: self.size,
            match_size: self.match_size
        }
    }
}


pub struct FMDIndex<'a> {
    fmindex: FMIndex<'a>,
    revcomp: dna::RevComp,
    interval_buffer: [BiInterval; 256],
}


impl<'a> FMDIndex<'a> {
    /// Construct a new instance of the FMD index (see Heng Li (2012) Bioinformatics).
    /// This expects a BWT that was created from a text over the DNA alphabet
    /// (ACGTN) consisting of the
    /// concatenation with its reverse complement, separated by the sentinel symbol `$`.
    /// I.e., let T be the original text and R be its reverse complement.
    /// Then, the expected text is T$R$. Further, multiple concatenated texts are allowed, e.g.
    /// T1$R1$T2$R2$T3$R3$.
    ///
    /// # Arguments
    ///
    /// * `bwt` - the BWT
    /// * `k` - the sampling rate of the occ array: every k-th entry will be stored (higher k means
    ///   less memory usage, but worse performance)
    pub fn new(bwt: &'a BWT, k: usize) -> Self {
        let alphabet = Alphabet::new(b"$ACGTN");
        assert!(
            alphabet.is_word(bwt),
            "Expecting BWT over the DNA alphabet (including N) with the sentinel $."
        );

        FMDIndex {
            fmindex: FMIndex::new(bwt, k, &alphabet),
            revcomp: dna::RevComp::new(),
            interval_buffer: [BiInterval { lower: 0, lower_rev: 0, size: 0, match_size: 0 }; 256]
        }
    }

    /// Find supermaximal exact matches of given pattern overlapping position i.
    ///
    /// # Example
    ///
    /// ```
    /// use bio::data_structures::fmindex::FMDIndex;
    /// use bio::data_structures::suffix_array::suffix_array;
    /// use bio::data_structures::bwt::bwt;
    ///
    /// let text = b"ATTC$GAAT$";
    /// let pos = suffix_array(text);
    /// let bwt = bwt(text, &pos);
    /// let mut fmdindex = FMDIndex::new(&bwt, 3);
    ///
    /// let pattern = b"ATT";
    /// let intervals = fmdindex.smems(pattern, 2);
    /// let occ = intervals[0].occ(&pos);
    /// let occ_revcomp = intervals[0].occ_revcomp(&pos);
    ///
    /// assert_eq!(occ, [0]);
    /// assert_eq!(occ_revcomp, [6]);
    /// ```
    pub fn smems(&mut self, pattern: &[u8], i: usize) -> Vec<BiInterval> {

        let curr = &mut Vec::new();
        let prev = &mut Vec::new();
        let mut matches = Vec::new();

        let mut interval = self.init_interval(pattern, i);

        for &a in pattern[i+1..].iter() {
            let _interval = self.forward_ext(&interval, a);

            if interval.size != _interval.size {
                curr.push(interval);
            }
            if _interval.size == 0 {
                break;
            }
            interval = _interval;
        }
        curr.push(interval);
        println!("{:?}", curr);

        swap(curr, prev);
        let mut j = pattern.len() as isize;

        for k in (-1..i as isize).rev() {
            let a = if k == -1 { b'$' } else { pattern[k as usize] };
            curr.clear();
            let mut s = -1;
            // iterate over forward extensions in reverse, as they are sorted by size
            // and we want longer matches first
            for &interval in prev.iter().rev() {
                let _interval = self.backward_ext(&interval, a);

                if _interval.size == 0 || k == -1 {
                    if curr.is_empty() && k < j {
                        j = k;
                        matches.push(interval);
                    }
                }
                // add _interval to curr (will be further extended next iteration
                if _interval.size != 0 && _interval.size != s {
                    s = _interval.size;
                    curr.push(_interval);
                }
            }
            if curr.is_empty() {
                break;
            }
            swap(curr, prev);
        }

        matches
    }

    fn init_interval(&self, pattern: &[u8], i: usize) -> BiInterval {
        let a = pattern[i];
        let _a = self.revcomp.comp(a);
        let lower = self.fmindex.less(a);

        BiInterval {
            lower: lower,
            lower_rev: self.fmindex.less(_a),
            size: self.fmindex.less(a + 1) - lower,
            match_size: 1,
        }
    }

    fn backward_ext(&mut self, interval: &BiInterval, a: u8) -> BiInterval {
        // calculate lower bound and size for all symbols
        for &b in b"$ACGTN".iter() {
            let o = self.fmindex.occ(interval.lower - 1, b);
            let b_interval = &mut self.interval_buffer[b as usize];
            b_interval.lower = self.fmindex.less(b) + o;
            b_interval.size = self.fmindex.occ(interval.lower + interval.size - 1, b) - o;
        }

        // calculate lower revcomp bounds
        {
            let sentinel_interval = &mut self.interval_buffer[b'$' as usize];
            sentinel_interval.lower_rev = interval.lower_rev;
        }

        let mut last = b'$' as usize;
        for &b in b"TGCAN".iter() {
            self.interval_buffer[b as usize].lower_rev = self.interval_buffer[last].lower_rev + self.interval_buffer[last].size;
            last = b as usize;
        }

        let mut ret = self.interval_buffer[a as usize];
        ret.match_size = interval.match_size + 1;

        ret
    }

    fn forward_ext(&mut self, interval: &BiInterval, a: u8) -> BiInterval {
        let _a = self.revcomp.comp(a);

        self.backward_ext(
            &interval.swapped(),
            _a
        ).swapped()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use alphabets::dna;
    use data_structures::suffix_array::suffix_array;
    use data_structures::bwt::bwt;

    #[test]
    fn test_smems() {
        let revcomp = dna::RevComp::new();
        let orig_text = b"GCCTTAACAT";
        let text = [orig_text, b"$", revcomp.get(orig_text).as_slice(), b"$"].concat();
        let pos = suffix_array(text.as_slice());
        println!("pos {:?}", pos);
        println!("text {:?}", text);
        let bwt = bwt(text.as_slice(), &pos);
        let mut fmdindex = FMDIndex::new(&bwt, 3);
        {
            let pattern = b"AA";
            let intervals = fmdindex.smems(pattern, 0);
            assert_eq!(intervals[0].occ(&pos), [5, 16]);
            assert_eq!(intervals[0].occ_revcomp(&pos), [3, 14]);
        }
        {
            let pattern = b"CTTAA";
            let intervals = fmdindex.smems(pattern, 1);
            assert_eq!(intervals[0].occ(&pos), [2]);
            assert_eq!(intervals[0].occ_revcomp(&pos), [14]);
            assert_eq!(intervals[0].match_size, 5)
        }
    }

    #[test]
    fn test_init_interval() {
        let text = b"ACGT$TGCA$";
        let pos = suffix_array(text);
        let bwt = bwt(text, &pos);
        let fmdindex = FMDIndex::new(&bwt, 3);
        let pattern = b"T";
        let interval = fmdindex.init_interval(pattern, 0);
        assert_eq!(interval.occ(&pos), [3, 5]);
        assert_eq!(interval.occ_revcomp(&pos), [8, 0]);
    }
}
