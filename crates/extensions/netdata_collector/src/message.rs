use std::convert::Infallible;
use std::vec;
use tedge_api::measurement::parse_str;
use tedge_api::measurement::MeasurementVisitor;
use tedge_api::measurement::ThinEdgeJsonParserError;

#[derive(Debug)]
pub struct MetricPoint {
    pub chart_id: String,
    pub dimension_id: String,
    pub value: i64,
}

#[derive(Debug)]
pub struct MetricPoints {
    points: Vec<MetricPoint>,
}

impl IntoIterator for MetricPoints {
    type Item = MetricPoint;
    type IntoIter = vec::IntoIter<MetricPoint>;

    fn into_iter(self) -> Self::IntoIter {
        self.points.into_iter()
    }
}

impl MetricPoints {
    pub fn parse(
        device: &str,
        measurement_type: &str,
        thin_edge_json: &str,
    ) -> Result<MetricPoints, ThinEdgeJsonParserError> {
        let mut builder = MetricPointsBuilder::new(device, measurement_type);
        parse_str(thin_edge_json, &mut builder)?;
        Ok(MetricPoints {
            points: builder.points,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &MetricPoint> {
        self.points.iter()
    }
}

struct MetricPointsBuilder {
    chart_id: String,
    points: Vec<MetricPoint>,
}

impl MetricPointsBuilder {
    fn new(device: &str, measurement_type: &str) -> Self {
        let chart_id = format!("tedge.{device}/{measurement_type}");
        MetricPointsBuilder {
            chart_id,
            points: vec![],
        }
    }
}

impl MeasurementVisitor for MetricPointsBuilder {
    type Error = Infallible;

    fn visit_timestamp(&mut self, _value: time::OffsetDateTime) -> Result<(), Self::Error> {
        // ignored: time is managed by netdata
        Ok(())
    }

    fn visit_measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        self.points.push(MetricPoint {
            chart_id: self.chart_id.clone(),
            dimension_id: name.to_string(),
            value: value.round() as i64,
        });
        Ok(())
    }

    fn visit_text_property(&mut self, _name: &str, _value: &str) -> Result<(), Self::Error> {
        // ignored: netdata only accepts number
        Ok(())
    }

    fn visit_json_property(
        &mut self,
        _name: &str,
        _value: serde_json::value::Value,
    ) -> Result<(), Self::Error> {
        // ignored: netdata only accepts number
        Ok(())
    }

    fn visit_start_group(&mut self, _group: &str) -> Result<(), Self::Error> {
        // ignored for now
        Ok(())
    }

    fn visit_end_group(&mut self) -> Result<(), Self::Error> {
        // ignored for now
        Ok(())
    }
}
