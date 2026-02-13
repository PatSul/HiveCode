use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// A single slide in a PPTX presentation.
pub struct PptxSlide {
    pub title: String,
    pub content: String,
}

/// Generate a PPTX (PowerPoint) file from a list of slides.
///
/// Each slide has a title and body content. The output is a valid OOXML
/// presentation file constructed using the `zip` crate.
pub fn generate_pptx(slides: &[PptxSlide]) -> Result<Vec<u8>> {
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // [Content_Types].xml
    zip.start_file("[Content_Types].xml", options)
        .context("Failed to create [Content_Types].xml")?;
    zip.write_all(content_types_xml(slides.len()).as_bytes())
        .context("Failed to write [Content_Types].xml")?;

    // _rels/.rels
    zip.start_file("_rels/.rels", options)
        .context("Failed to create _rels/.rels")?;
    zip.write_all(root_rels_xml().as_bytes())
        .context("Failed to write _rels/.rels")?;

    // ppt/presentation.xml
    zip.start_file("ppt/presentation.xml", options)
        .context("Failed to create ppt/presentation.xml")?;
    zip.write_all(presentation_xml(slides.len()).as_bytes())
        .context("Failed to write ppt/presentation.xml")?;

    // ppt/_rels/presentation.xml.rels
    zip.start_file("ppt/_rels/presentation.xml.rels", options)
        .context("Failed to create ppt/_rels/presentation.xml.rels")?;
    zip.write_all(presentation_rels_xml(slides.len()).as_bytes())
        .context("Failed to write ppt/_rels/presentation.xml.rels")?;

    // ppt/slideMasters/slideMaster1.xml
    zip.start_file("ppt/slideMasters/slideMaster1.xml", options)
        .context("Failed to create slideMaster1.xml")?;
    zip.write_all(slide_master_xml().as_bytes())
        .context("Failed to write slideMaster1.xml")?;

    // ppt/slideMasters/_rels/slideMaster1.xml.rels
    zip.start_file("ppt/slideMasters/_rels/slideMaster1.xml.rels", options)
        .context("Failed to create slideMaster1.xml.rels")?;
    zip.write_all(slide_master_rels_xml().as_bytes())
        .context("Failed to write slideMaster1.xml.rels")?;

    // ppt/slideLayouts/slideLayout1.xml
    zip.start_file("ppt/slideLayouts/slideLayout1.xml", options)
        .context("Failed to create slideLayout1.xml")?;
    zip.write_all(slide_layout_xml().as_bytes())
        .context("Failed to write slideLayout1.xml")?;

    // ppt/slideLayouts/_rels/slideLayout1.xml.rels
    zip.start_file("ppt/slideLayouts/_rels/slideLayout1.xml.rels", options)
        .context("Failed to create slideLayout1.xml.rels")?;
    zip.write_all(slide_layout_rels_xml().as_bytes())
        .context("Failed to write slideLayout1.xml.rels")?;

    // ppt/theme/theme1.xml
    zip.start_file("ppt/theme/theme1.xml", options)
        .context("Failed to create theme1.xml")?;
    zip.write_all(theme_xml().as_bytes())
        .context("Failed to write theme1.xml")?;

    // Individual slides
    for (i, slide) in slides.iter().enumerate() {
        let slide_num = i + 1;

        // ppt/slides/slideN.xml
        let slide_path = format!("ppt/slides/slide{slide_num}.xml");
        zip.start_file(&slide_path, options)
            .with_context(|| format!("Failed to create {slide_path}"))?;
        zip.write_all(slide_xml(&slide.title, &slide.content).as_bytes())
            .with_context(|| format!("Failed to write {slide_path}"))?;

        // ppt/slides/_rels/slideN.xml.rels
        let slide_rels_path = format!("ppt/slides/_rels/slide{slide_num}.xml.rels");
        zip.start_file(&slide_rels_path, options)
            .with_context(|| format!("Failed to create {slide_rels_path}"))?;
        zip.write_all(slide_rels_xml().as_bytes())
            .with_context(|| format!("Failed to write {slide_rels_path}"))?;
    }

    let cursor = zip.finish().context("Failed to finalize PPTX zip")?;
    Ok(cursor.into_inner())
}

// ---------------------------------------------------------------------------
// XML template functions
// ---------------------------------------------------------------------------

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn content_types_xml(slide_count: usize) -> String {
    let mut overrides = String::new();
    for i in 1..=slide_count {
        overrides.push_str(&format!(
            r#"  <Override PartName="/ppt/slides/slide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#
        ));
        overrides.push('\n');
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
  <Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
  <Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
{overrides}</Types>"#
    )
}

fn root_rels_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#
        .to_string()
}

fn presentation_xml(slide_count: usize) -> String {
    let mut slide_list = String::new();
    for i in 1..=slide_count {
        slide_list.push_str(&format!(
            r#"    <p:sldId id="{}" r:id="rId{}"/>"#,
            255 + i,
            i + 2 // rId1=slideMaster, rId2=theme, slides start at rId3
        ));
        slide_list.push('\n');
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldMasterIdLst>
    <p:sldMasterId id="2147483648" r:id="rId1"/>
  </p:sldMasterIdLst>
  <p:sldIdLst>
{slide_list}  </p:sldIdLst>
  <p:sldSz cx="9144000" cy="6858000" type="screen4x3"/>
  <p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#
    )
}

fn presentation_rels_xml(slide_count: usize) -> String {
    let mut rels = String::new();
    // rId1 = slideMaster
    rels.push_str(
        r#"  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#,
    );
    rels.push('\n');
    // rId2 = theme
    rels.push_str(
        r#"  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>"#,
    );
    rels.push('\n');
    // rId3+ = slides
    for i in 1..=slide_count {
        rels.push_str(&format!(
            r#"  <Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{i}.xml"/>"#,
            i + 2
        ));
        rels.push('\n');
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
{rels}</Relationships>"#
    )
}

fn slide_master_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr/>
    </p:spTree>
  </p:cSld>
  <p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
  <p:sldLayoutIdLst>
    <p:sldLayoutId id="2147483649" r:id="rId1"/>
  </p:sldLayoutIdLst>
</p:sldMaster>"#
        .to_string()
}

fn slide_master_rels_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
</Relationships>"#
        .to_string()
}

fn slide_layout_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr/>
    </p:spTree>
  </p:cSld>
</p:sldLayout>"#
        .to_string()
}

fn slide_layout_rels_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#
        .to_string()
}

fn theme_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Default Theme">
  <a:themeElements>
    <a:clrScheme name="Default">
      <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
      <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
      <a:dk2><a:srgbClr val="44546A"/></a:dk2>
      <a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
      <a:accent1><a:srgbClr val="4472C4"/></a:accent1>
      <a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
      <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>
      <a:accent4><a:srgbClr val="FFC000"/></a:accent4>
      <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5>
      <a:accent6><a:srgbClr val="70AD47"/></a:accent6>
      <a:hlink><a:srgbClr val="0563C1"/></a:hlink>
      <a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
    </a:clrScheme>
    <a:fontScheme name="Default">
      <a:majorFont><a:latin typeface="Calibri"/></a:majorFont>
      <a:minorFont><a:latin typeface="Calibri"/></a:minorFont>
    </a:fontScheme>
    <a:fmtScheme name="Default">
      <a:fillStyleLst>
        <a:solidFill><a:schemeClr val="phClr"/></a:solidFill>
        <a:solidFill><a:schemeClr val="phClr"/></a:solidFill>
        <a:solidFill><a:schemeClr val="phClr"/></a:solidFill>
      </a:fillStyleLst>
      <a:lnStyleLst>
        <a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>
        <a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>
        <a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>
      </a:lnStyleLst>
      <a:effectStyleLst>
        <a:effectStyle><a:effectLst/></a:effectStyle>
        <a:effectStyle><a:effectLst/></a:effectStyle>
        <a:effectStyle><a:effectLst/></a:effectStyle>
      </a:effectStyleLst>
      <a:bgFillStyleLst>
        <a:solidFill><a:schemeClr val="phClr"/></a:solidFill>
        <a:solidFill><a:schemeClr val="phClr"/></a:solidFill>
        <a:solidFill><a:schemeClr val="phClr"/></a:solidFill>
      </a:bgFillStyleLst>
    </a:fmtScheme>
  </a:themeElements>
</a:theme>"#
        .to_string()
}

fn slide_xml(title: &str, content: &str) -> String {
    let escaped_title = xml_escape(title);
    let escaped_content = xml_escape(content);

    // Build content paragraphs -- each line becomes a separate <a:p> element
    let mut content_paragraphs = String::new();
    for line in escaped_content.lines() {
        content_paragraphs.push_str(&format!(
            r#"              <a:p>
                <a:r>
                  <a:rPr lang="en-US" sz="1800" dirty="0"/>
                  <a:t>{line}</a:t>
                </a:r>
              </a:p>"#
        ));
        content_paragraphs.push('\n');
    }
    // If content is empty, add an empty paragraph
    if content.is_empty() {
        content_paragraphs.push_str("              <a:p><a:endParaRPr lang=\"en-US\"/></a:p>\n");
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr/>
      <p:sp>
        <p:nvSpPr>
          <p:cNvPr id="2" name="Title"/>
          <p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr>
          <p:nvPr><p:ph type="title"/></p:nvPr>
        </p:nvSpPr>
        <p:spPr>
          <a:xfrm>
            <a:off x="457200" y="274638"/>
            <a:ext cx="8229600" cy="1143000"/>
          </a:xfrm>
        </p:spPr>
        <p:txBody>
          <a:bodyPr/>
          <a:lstStyle/>
          <a:p>
            <a:r>
              <a:rPr lang="en-US" sz="3600" b="1" dirty="0"/>
              <a:t>{escaped_title}</a:t>
            </a:r>
          </a:p>
        </p:txBody>
      </p:sp>
      <p:sp>
        <p:nvSpPr>
          <p:cNvPr id="3" name="Content"/>
          <p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr>
          <p:nvPr><p:ph idx="1"/></p:nvPr>
        </p:nvSpPr>
        <p:spPr>
          <a:xfrm>
            <a:off x="457200" y="1600200"/>
            <a:ext cx="8229600" cy="4525963"/>
          </a:xfrm>
        </p:spPr>
        <p:txBody>
          <a:bodyPr/>
          <a:lstStyle/>
{content_paragraphs}        </p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
</p:sld>"#
    )
}

fn slide_rels_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_pptx_single_slide() {
        let slides = vec![PptxSlide {
            title: "Welcome".to_string(),
            content: "Hello, World!".to_string(),
        }];
        let bytes = generate_pptx(&slides).unwrap();
        // PPTX is a zip file -- starts with PK magic bytes
        assert_eq!(&bytes[0..2], b"PK");
        assert!(bytes.len() > 100);
    }

    #[test]
    fn test_generate_pptx_multiple_slides() {
        let slides = vec![
            PptxSlide {
                title: "Slide 1".to_string(),
                content: "First slide content".to_string(),
            },
            PptxSlide {
                title: "Slide 2".to_string(),
                content: "Second slide content".to_string(),
            },
            PptxSlide {
                title: "Slide 3".to_string(),
                content: "Third slide content".to_string(),
            },
        ];
        let bytes = generate_pptx(&slides).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
        assert!(bytes.len() > 500);
    }

    #[test]
    fn test_generate_pptx_empty_slides() {
        let slides: Vec<PptxSlide> = vec![];
        let bytes = generate_pptx(&slides).unwrap();
        // Even with no slides, must be valid zip
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_pptx_special_characters() {
        let slides = vec![PptxSlide {
            title: "Symbols & <Signs>".to_string(),
            content: "Price: $100 \"quoted\" & 'apostrophe'".to_string(),
        }];
        let bytes = generate_pptx(&slides).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_pptx_multiline_content() {
        let slides = vec![PptxSlide {
            title: "Bullet Points".to_string(),
            content: "First point\nSecond point\nThird point".to_string(),
        }];
        let bytes = generate_pptx(&slides).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
        assert!(bytes.len() > 100);
    }

    #[test]
    fn test_generate_pptx_empty_content() {
        let slides = vec![PptxSlide {
            title: "Title Only".to_string(),
            content: String::new(),
        }];
        let bytes = generate_pptx(&slides).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_pptx_is_valid_zip() {
        let slides = vec![PptxSlide {
            title: "Test".to_string(),
            content: "Content".to_string(),
        }];
        let bytes = generate_pptx(&slides).unwrap();

        // Verify we can read it back as a zip
        let cursor = Cursor::new(&bytes);
        let archive = zip::ZipArchive::new(cursor).unwrap();

        // Check expected files exist
        let names: Vec<&str> = archive.file_names().collect();
        assert!(names.contains(&"[Content_Types].xml"));
        assert!(names.contains(&"_rels/.rels"));
        assert!(names.contains(&"ppt/presentation.xml"));
        assert!(names.contains(&"ppt/slides/slide1.xml"));
        assert!(names.contains(&"ppt/slideMasters/slideMaster1.xml"));
        assert!(names.contains(&"ppt/slideLayouts/slideLayout1.xml"));
        assert!(names.contains(&"ppt/theme/theme1.xml"));
    }
}
